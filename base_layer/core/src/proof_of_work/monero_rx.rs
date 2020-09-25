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
use log::*;
use monero::{
    blockdata::{
        block::{Block as MoneroBlock, BlockHeader as MoneroBlockHeader},
        transaction::SubField,
        Transaction as MoneroTransaction,
    },
    consensus::{encode::VarInt, serialize},
    cryptonote::hash::{Hash, Hashable},
};
use randomx_rs::{RandomXCache, RandomXDataset, RandomXError, RandomXFlag, RandomXVM};
use serde::{Deserialize, Serialize};
use std::{
    fmt::{Display, Error, Formatter},
    iter,
};
use tari_crypto::tari_utilities::hex::Hex;
use thiserror::Error;

const MAX_TARGET: U256 = U256::MAX;
pub const LOG_TARGET: &str = "c::pow::monero_rx";

#[derive(Debug, Error)]
pub enum MergeMineError {
    #[error("Serialization error: {0}")]
    SerializeError(String),
    #[error("Error deserializing Monero data: {0}")]
    DeserializeError(String),
    #[error("Hashing of Monero data failed: {0}")]
    HashingError(String),
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

impl Display for MoneroData {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        writeln!(fmt, "MoneroBlockHeader: {} ", self.header)?;
        writeln!(fmt, "RandomX vm key: {}", self.key)?;
        writeln!(fmt, "Monero tx count: {}", self.count.to_string())?;
        writeln!(fmt, "Monero tx root: {}", self.transaction_root.to_hex())?;
        writeln!(fmt, "Monero coinbase tx: {}", self.coinbase_tx)
    }
}

// Hash algorithm in monero
pub fn cn_fast_hash(data: &[u8]) -> Hash {
    Hash::hash(data)
}

// Tree hash count in monero
fn tree_hash_cnt(count: usize) -> Result<usize, MergeMineError> {
    if count < 3 {
        return Err(MergeMineError::HashingError(
            "Cannot calculate Monero root, algorithm path error".to_string(),
        ));
    }

    if count > 0x10000000 {
        return Err(MergeMineError::HashingError(
            "Cannot calculate Monero root, hash count too large".to_string(),
        ));
    }

    let mut pow: usize = 2;
    while pow < count {
        pow <<= 1;
    }

    Ok(pow >> 1)
}

/// Tree hash algorithm in monero
#[allow(clippy::needless_range_loop)]
pub fn tree_hash(hashes: &[Hash]) -> Result<Hash, MergeMineError> {
    if hashes.is_empty() {
        return Err(MergeMineError::HashingError(
            "Cannot calculate Monero root, hashes is empty".to_string(),
        ));
    }

    match hashes.len() {
        1 => Ok(hashes[0]),
        2 => {
            let mut buf: [u8; 64] = [0; 64];
            buf[..32].copy_from_slice(&hashes[0].0.to_vec());
            buf[32..].copy_from_slice(&hashes[1].0.to_vec());
            Ok(cn_fast_hash(&buf))
        },
        _ => {
            let mut cnt = tree_hash_cnt(hashes.len())?;
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
                buf[(j * 32)..((j + 1) * 32)].copy_from_slice(&tmp.0);
                i += 2;
            }

            if !(i == hashes.len()) {
                return Err(MergeMineError::HashingError(
                    "Cannot calculate Monero root, hashes not equal to count".to_string(),
                ));
            }

            while cnt > 2 {
                cnt >>= 1;
                let mut i = 0;
                for j in (0..(cnt * 32)).step_by(32) {
                    let tmp = cn_fast_hash(&buf[i..(i + 64)]);
                    buf[j..(j + 32)].copy_from_slice(&tmp.0);
                    i += 64;
                }
            }

            Ok(cn_fast_hash(&buf[..64]))
        },
    }
}

impl MoneroData {
    pub fn new(tari_header: &BlockHeader) -> Result<MoneroData, MergeMineError> {
        bincode::deserialize(&tari_header.pow.pow_data).map_err(|e| MergeMineError::DeserializeError(e.to_string()))
    }

    pub fn new_from_pow(pow_data: &Vec<u8>) -> Result<MoneroData, MergeMineError> {
        bincode::deserialize(pow_data).map_err(|e| MergeMineError::DeserializeError(e.to_string()))
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
    let MoneroData {
        key,
        transaction_hashes,
        ..
    } = monero;

    let tx_hashes = transaction_hashes.iter().map(Into::into).collect::<Vec<_>>();
    let input = create_input_blob_from_parts(&monero.header, &tx_hashes)?;
    let cache = RandomXCache::new(flags, (&key).as_ref())?;
    let dataset = RandomXDataset::new(flags, &cache, 0)?;
    let vm = RandomXVM::new(flags, Some(&cache), Some(&dataset))?;
    let hash = vm.calculate_hash((&input).as_ref())?;
    let scalar = U256::from_big_endian(&hash); // Big endian so the hash has leading zeroes
    let result = MAX_TARGET / scalar;
    let difficulty = Difficulty::from(result.low_u64());
    Ok(difficulty)
}

/// Appends merge mining hash to a Monero block
pub fn append_merge_mining_tag<T: AsRef<[u8]>>(block: &mut MoneroBlock, hash: T) -> Result<(), MergeMineError> {
    if hash.as_ref().len() != Hash::len_bytes() {
        return Err(MergeMineError::HashingError(format!(
            "Expected source to be {} bytes, but it was {} bytes",
            Hash::len_bytes(),
            hash.as_ref().len()
        )));
    }
    let hash = Hash::from_slice(hash.as_ref());
    let mm_tag = SubField::MergeMining(VarInt(0), hash);
    block.miner_tx.prefix.extra.0.push(mm_tag);
    Ok(())
}

/// Creates a hex encoded Monero blockhashing_blob
pub fn create_input_blob(block: &MoneroBlock) -> Result<String, MergeMineError> {
    let tx_hashes = create_ordered_transaction_hashes_from_block(block);
    create_input_blob_from_parts(&block.header, &tx_hashes)
}

pub fn create_ordered_transaction_hashes_from_block(block: &MoneroBlock) -> Vec<Hash> {
    iter::once(block.miner_tx.hash())
        .chain(block.tx_hashes.clone())
        .collect()
}

/// Creates a hex encoded Monero blockhashing_blob
fn create_input_blob_from_parts(header: &MoneroBlockHeader, tx_hashes: &[Hash]) -> Result<String, MergeMineError> {
    let header = serialize::<MoneroBlockHeader>(&header);
    let mut encode = header;

    let root = tree_hash(tx_hashes)?;
    encode.extend_from_slice(root.as_bytes());

    let mut count = serialize(&VarInt(tx_hashes.len() as u64));
    encode.append(&mut count);
    Ok(hex::encode(encode))
}

/// Utility function to transform array of hash to fixed array of [u8; 32]
pub fn from_hashes_to_array<T: IntoIterator<Item = Hash>>(hashes: T) -> Vec<[u8; 32]> {
    hashes.into_iter().map(|h| h.to_fixed_bytes()).collect()
}

fn verify_root(monero_data: &MoneroData) -> Result<(), MergeMineError> {
    // let mut hashes = Vec::with_capacity(monero_data.transaction_hashes.len());
    // for item in &monero_data.transaction_hashes {
    //     hashes.push(Hash::from(item));
    // }
    let hashes = monero_data
        .transaction_hashes
        .iter()
        .map(Into::into)
        .collect::<Vec<_>>();
    let root = tree_hash(&hashes)?;

    if !(&monero_data.transaction_root == root.as_fixed_bytes()) {
        return Err(MergeMineError::ValidationError(
            "Transaction root did not match".to_string(),
        ));
    }
    Ok(())
}

fn verify_header(header: &BlockHeader, monero_data: &MoneroData) -> Result<(), MergeMineError> {
    let expected_merge_mining_hash = header.merged_mining_hash();

    let is_found = monero_data.coinbase_tx.prefix.extra.0.iter().any(|item| match item {
        SubField::MergeMining(depth, merge_mining_hash) => {
            depth == &VarInt(0) && merge_mining_hash.as_bytes() == expected_merge_mining_hash.as_slice()
        },
        _ => false,
    });

    if !is_found {
        return Err(MergeMineError::ValidationError(
            "Expected merge mining tag was not found in Monero coinbase transaction".to_string(),
        ));
    }
    verify_root(monero_data)?;

    // TODO: add seed check here.
    Ok(())
}

#[cfg(test)]
mod test {
    use crate::{
        blocks::BlockHeader,
        proof_of_work::{
            monero_difficulty,
            monero_rx::{
                append_merge_mining_tag,
                create_input_blob,
                create_ordered_transaction_hashes_from_block,
                from_hashes_to_array,
                tree_hash,
                verify_header,
                MergeMineError,
                MoneroData,
            },
            PowAlgorithm,
            ProofOfWork,
        },
        tari_utilities::ByteArray,
    };
    use monero::{
        blockdata::{
            block::BlockHeader as MoneroHeader,
            transaction::{ExtraField, SubField, TxOutTarget},
            Block as MoneroBlock,
            TransactionPrefix,
            TxIn,
        },
        consensus::{deserialize, encode::VarInt, serialize},
        cryptonote::hash::{Hash, Hashable},
        util::ringct::{RctSig, RctSigBase, RctType},
        PublicKey,
        Transaction,
        TxOut,
    };
    use tari_crypto::ristretto::RistrettoSecretKey;
    use tari_test_utils::unpack_enum;

    // This tests checks the hash of monero-rs
    #[test]
    fn test_monero_rs_miner_tx_hash() {
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
    fn test_monero_rs_block_serialize() {
        // block with only the miner tx and no other transactions
        let hex = "0c0c94debaf805beb3489c722a285c092a32e7c6893abfc7d069699c8326fc3445a749c5276b6200000000029b892201ffdf882201b699d4c8b1ec020223df524af2a2ef5f870adb6e1ceb03a475c39f8b9ef76aa50b46ddd2a18349402b012839bfa19b7524ec7488917714c216ca254b38ed0424ca65ae828a7c006aeaf10208f5316a7f6b99cca60000";
        // blockhashing blob for above block as accepted by monero
        let hex_blockhash_blob="0c0c94debaf805beb3489c722a285c092a32e7c6893abfc7d069699c8326fc3445a749c5276b6200000000602d0d4710e2c2d38da0cce097accdf5dc18b1d34323880c1aae90ab8f6be6e201";
        let bytes = hex::decode(hex).unwrap();
        let block = deserialize::<MoneroBlock>(&bytes[..]).unwrap();
        let header = serialize::<MoneroHeader>(&block.header);
        let tx_count = 1 + block.tx_hashes.len() as u64;
        let mut count = serialize::<VarInt>(&VarInt(tx_count));
        let mut hashes = Vec::with_capacity(tx_count as usize);
        hashes.push(block.miner_tx.hash());
        for item in block.clone().tx_hashes {
            hashes.push(item);
        }
        let root = tree_hash(&hashes).unwrap();
        let mut encode2 = header;
        encode2.extend_from_slice(root.as_bytes());
        encode2.append(&mut count);
        assert_eq!(hex::encode(encode2), hex_blockhash_blob);
        let bytes2 = serialize::<MoneroBlock>(&block);
        assert_eq!(bytes, bytes2);
        let hex2 = hex::encode(bytes2);
        assert_eq!(hex, hex2);
    }

    #[test]
    fn test_monero_data() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<MoneroBlock>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: vec![0],
            timestamp: Default::default(),
            output_mr: vec![0],
            range_proof_mr: vec![0],
            kernel_mr: vec![0],
            total_kernel_offset: RistrettoSecretKey::from(0),
            nonce: 0,
            pow: ProofOfWork::default(),
        };
        let hash = block_header.merged_mining_hash();
        append_merge_mining_tag(&mut block, hash).unwrap();
        let hashes = create_ordered_transaction_hashes_from_block(&block);
        let root = tree_hash(&hashes).unwrap();
        let monero_data = MoneroData {
            header: block.header,
            key: seed_hash,
            count: hashes.len() as u16,
            transaction_root: root.to_fixed_bytes(),
            transaction_hashes: hashes.into_iter().map(|h| h.to_fixed_bytes()).collect(),
            coinbase_tx: block.miner_tx,
        };
        let serialized = bincode::serialize(&monero_data).unwrap();
        let pow = ProofOfWork {
            accumulated_monero_difficulty: Default::default(),
            accumulated_blake_difficulty: Default::default(),
            target_difficulty: Default::default(),
            pow_algo: PowAlgorithm::Monero,
            pow_data: serialized,
        };
        block_header.pow = pow;
        MoneroData::new(&block_header).unwrap();
    }

    #[test]
    fn test_tree_hash() {
        let tx_hash = [
            88, 176, 48, 182, 128, 13, 67, 59, 188, 178, 181, 96, 175, 226, 160, 142, 77, 193, 82, 250, 119, 234, 217,
            109, 55, 170, 241, 72, 151, 211, 192, 150,
        ];
        let mut hashes = Vec::new();
        hashes.push(Hash::from(tx_hash));
        let mut root = tree_hash(&hashes).unwrap();
        assert_eq!(root.as_bytes(), tx_hash);
        hashes.push(Hash::from(tx_hash));
        root = tree_hash(&hashes).unwrap();
        let mut correct_root = [
            187, 251, 201, 6, 70, 27, 80, 117, 95, 97, 244, 143, 194, 245, 73, 174, 158, 255, 98, 175, 74, 22, 173,
            223, 217, 17, 59, 183, 230, 39, 76, 202,
        ];
        assert_eq!(root.as_bytes(), correct_root);

        hashes.push(Hash::from(tx_hash));
        root = tree_hash(&hashes).unwrap();
        correct_root = [
            37, 100, 243, 131, 133, 33, 135, 169, 23, 215, 243, 10, 213, 152, 21, 10, 89, 86, 217, 49, 245, 237, 205,
            194, 102, 162, 128, 225, 215, 192, 158, 251,
        ];
        assert_eq!(root.as_bytes(), correct_root);

        hashes.push(Hash::from(tx_hash));
        root = tree_hash(&hashes).unwrap();
        correct_root = [
            52, 199, 248, 213, 213, 138, 52, 0, 145, 179, 81, 247, 174, 31, 183, 196, 124, 186, 100, 21, 36, 252, 171,
            66, 250, 247, 122, 64, 36, 127, 184, 46,
        ];
        assert_eq!(root.as_bytes(), correct_root);
    }

    #[test]
    fn test_tree_hash_fail() {
        let hashes = Vec::new();
        let err = tree_hash(&hashes).unwrap_err();
        unpack_enum!(MergeMineError::HashingError(details) = err);
        assert!(details.contains("Cannot calculate Monero root, hashes is empty"));
    }

    #[test]
    fn test_input_blob() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let block = deserialize::<MoneroBlock>(&bytes[..]).unwrap();
        let input_blob = create_input_blob(&block).unwrap();
        assert_eq!(input_blob, "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000058b030b6800d433bbcb2b560afe2a08e4dc152fa77ead96d37aaf14897d3c09601");
    }

    #[test]
    fn test_append_mm_tag() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<MoneroBlock>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: vec![0],
            timestamp: Default::default(),
            output_mr: vec![0],
            range_proof_mr: vec![0],
            kernel_mr: vec![0],
            total_kernel_offset: RistrettoSecretKey::from(0),
            nonce: 0,
            pow: ProofOfWork::default(),
        };
        let hash = block_header.merged_mining_hash();
        append_merge_mining_tag(&mut block, hash).unwrap();
        let count = 1 + (block.tx_hashes.len() as u16);
        let mut hashes = Vec::with_capacity(count as usize);
        let mut proof = Vec::with_capacity(count as usize);
        hashes.push(block.miner_tx.hash());
        proof.push(block.miner_tx.hash());
        for item in block.clone().tx_hashes {
            hashes.push(item);
            proof.push(item);
        }
        let root = tree_hash(&hashes).unwrap();
        let monero_data = MoneroData {
            header: block.header,
            key: seed_hash,
            count,
            transaction_root: root.to_fixed_bytes(),
            transaction_hashes: from_hashes_to_array(hashes),
            coinbase_tx: block.miner_tx,
        };
        let serialized = bincode::serialize(&monero_data).unwrap();
        let pow = ProofOfWork {
            accumulated_monero_difficulty: Default::default(),
            accumulated_blake_difficulty: Default::default(),
            target_difficulty: Default::default(),
            pow_algo: PowAlgorithm::Monero,
            pow_data: serialized,
        };
        block_header.pow = pow;
        let monero_data = MoneroData::new(&block_header).unwrap();
        verify_header(&block_header, &monero_data).unwrap();
    }

    #[test]
    fn test_append_mm_tag_no_tag() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let block = deserialize::<MoneroBlock>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: vec![0],
            timestamp: Default::default(),
            output_mr: vec![0],
            range_proof_mr: vec![0],
            kernel_mr: vec![0],
            total_kernel_offset: RistrettoSecretKey::from(0),
            nonce: 0,
            pow: ProofOfWork::default(),
        };
        let count = 1 + (block.tx_hashes.len() as u16);
        let mut hashes = Vec::with_capacity(count as usize);
        let mut proof = Vec::with_capacity(count as usize);
        hashes.push(block.miner_tx.hash());
        proof.push(block.miner_tx.hash());
        for item in block.clone().tx_hashes {
            hashes.push(item);
            proof.push(item);
        }
        let root = tree_hash(&hashes).unwrap();
        let monero_data = MoneroData {
            header: block.header,
            key: seed_hash,
            count,
            transaction_root: root.to_fixed_bytes(),
            transaction_hashes: from_hashes_to_array(hashes),
            coinbase_tx: block.miner_tx,
        };
        let serialized = bincode::serialize(&monero_data).unwrap();
        let pow = ProofOfWork {
            accumulated_monero_difficulty: Default::default(),
            accumulated_blake_difficulty: Default::default(),
            target_difficulty: Default::default(),
            pow_algo: PowAlgorithm::Monero,
            pow_data: serialized,
        };
        block_header.pow = pow;
        let monero_data = MoneroData::new(&block_header).unwrap();
        let err = verify_header(&block_header, &monero_data).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("Expected merge mining tag was not found in Monero coinbase transaction"));
    }

    #[test]
    fn test_append_mm_tag_wrong_hash() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<MoneroBlock>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: vec![0],
            timestamp: Default::default(),
            output_mr: vec![0],
            range_proof_mr: vec![0],
            kernel_mr: vec![0],
            total_kernel_offset: RistrettoSecretKey::from(0),
            nonce: 0,
            pow: ProofOfWork::default(),
        };
        let hash = Hash::null_hash();
        append_merge_mining_tag(&mut block, hash).unwrap();
        let count = 1 + (block.tx_hashes.len() as u16);
        let mut hashes = Vec::with_capacity(count as usize);
        let mut proof = Vec::with_capacity(count as usize);
        hashes.push(block.miner_tx.hash());
        proof.push(block.miner_tx.hash());
        for item in block.clone().tx_hashes {
            hashes.push(item);
            proof.push(item);
        }
        let root = tree_hash(&hashes).unwrap();
        let monero_data = MoneroData {
            header: block.header,
            key: seed_hash,
            count,
            transaction_root: root.to_fixed_bytes(),
            transaction_hashes: from_hashes_to_array(hashes),
            coinbase_tx: block.miner_tx,
        };
        let serialized = bincode::serialize(&monero_data).unwrap();
        let pow = ProofOfWork {
            accumulated_monero_difficulty: Default::default(),
            accumulated_blake_difficulty: Default::default(),
            target_difficulty: Default::default(),
            pow_algo: PowAlgorithm::Monero,
            pow_data: serialized,
        };
        block_header.pow = pow;
        let monero_data = MoneroData::new(&block_header).unwrap();
        let err = verify_header(&block_header, &monero_data).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("Expected merge mining tag was not found in Monero coinbase transaction"));
    }

    #[test]
    fn test_verify_header_no_coinbase() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<MoneroBlock>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: vec![0],
            timestamp: Default::default(),
            output_mr: vec![0],
            range_proof_mr: vec![0],
            kernel_mr: vec![0],
            total_kernel_offset: RistrettoSecretKey::from(0),
            nonce: 0,
            pow: ProofOfWork::default(),
        };
        let hash = block_header.merged_mining_hash();
        append_merge_mining_tag(&mut block, hash).unwrap();
        let count = 1 + (block.tx_hashes.len() as u16);
        let mut hashes = Vec::with_capacity(count as usize);
        let mut proof = Vec::with_capacity(count as usize);
        hashes.push(block.miner_tx.hash());
        proof.push(block.miner_tx.hash());
        for item in block.clone().tx_hashes {
            hashes.push(item);
            proof.push(item);
        }
        let root = tree_hash(&hashes).unwrap();
        let monero_data = MoneroData {
            header: block.header,
            key: seed_hash,
            count,
            transaction_root: root.to_fixed_bytes(),
            transaction_hashes: from_hashes_to_array(hashes),
            coinbase_tx: Default::default(),
        };
        let serialized = bincode::serialize(&monero_data).unwrap();
        let pow = ProofOfWork {
            accumulated_monero_difficulty: Default::default(),
            accumulated_blake_difficulty: Default::default(),
            target_difficulty: Default::default(),
            pow_algo: PowAlgorithm::Monero,
            pow_data: serialized,
        };
        block_header.pow = pow;
        let monero_data = MoneroData::new(&block_header).unwrap();
        let err = verify_header(&block_header, &monero_data).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("Expected merge mining tag was not found in Monero coinbase transaction"));
    }

    #[test]
    fn test_verify_header_no_data() {
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: vec![0],
            timestamp: Default::default(),
            output_mr: vec![0],
            range_proof_mr: vec![0],
            kernel_mr: vec![0],
            total_kernel_offset: RistrettoSecretKey::from(0),
            nonce: 0,
            pow: ProofOfWork::default(),
        };
        let monero_data = MoneroData::default();
        let serialized = bincode::serialize(&monero_data).unwrap();
        let pow = ProofOfWork {
            accumulated_monero_difficulty: Default::default(),
            accumulated_blake_difficulty: Default::default(),
            target_difficulty: Default::default(),
            pow_algo: PowAlgorithm::Monero,
            pow_data: serialized,
        };
        block_header.pow = pow;
        let monero_data = MoneroData::new(&block_header).unwrap();
        let err = verify_header(&block_header, &monero_data).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("Expected merge mining tag was not found in Monero coinbase transaction"));
    }

    #[test]
    fn test_varify_invalid_root() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<MoneroBlock>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: vec![0],
            timestamp: Default::default(),
            output_mr: vec![0],
            range_proof_mr: vec![0],
            kernel_mr: vec![0],
            total_kernel_offset: RistrettoSecretKey::from(0),
            nonce: 0,
            pow: ProofOfWork::default(),
        };
        let hash = block_header.merged_mining_hash();
        append_merge_mining_tag(&mut block, hash).unwrap();
        let count = 1 + (block.tx_hashes.len() as u16);
        let mut hashes = Vec::with_capacity(count as usize);
        let mut proof = Vec::with_capacity(count as usize);
        hashes.push(block.miner_tx.hash());
        proof.push(block.miner_tx.hash());
        for item in block.clone().tx_hashes {
            hashes.push(item);
            proof.push(item);
        }
        let monero_data = MoneroData {
            header: block.header,
            key: seed_hash,
            count,
            transaction_root: Hash::null_hash().0,
            transaction_hashes: from_hashes_to_array(hashes),
            coinbase_tx: block.miner_tx,
        };
        let serialized = bincode::serialize(&monero_data).unwrap();
        let pow = ProofOfWork {
            accumulated_monero_difficulty: Default::default(),
            accumulated_blake_difficulty: Default::default(),
            target_difficulty: Default::default(),
            pow_algo: PowAlgorithm::Monero,
            pow_data: serialized,
        };
        block_header.pow = pow;
        let monero_data = MoneroData::new(&block_header).unwrap();
        let err = verify_header(&block_header, &monero_data).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("Transaction root did not match"));
    }

    #[test]
    fn test_difficulty_calculation() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<MoneroBlock>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: vec![0],
            timestamp: Default::default(),
            output_mr: vec![0],
            range_proof_mr: vec![0],
            kernel_mr: vec![0],
            total_kernel_offset: RistrettoSecretKey::from(0),
            nonce: 0,
            pow: ProofOfWork::default(),
        };
        let hash = block_header.merged_mining_hash();
        append_merge_mining_tag(&mut block, hash).unwrap();
        let count = 1 + (block.tx_hashes.len() as u16);
        let mut hashes = Vec::with_capacity(count as usize);
        let mut proof = Vec::with_capacity(count as usize);
        hashes.push(block.miner_tx.hash());
        proof.push(block.miner_tx.hash());
        for item in block.clone().tx_hashes {
            hashes.push(item);
            proof.push(item);
        }
        let root = tree_hash(&hashes).unwrap();
        let monero_data = MoneroData {
            header: block.header,
            key: seed_hash,
            count,
            transaction_root: root.to_fixed_bytes(),
            transaction_hashes: from_hashes_to_array(hashes),
            coinbase_tx: block.miner_tx,
        };
        let serialized = bincode::serialize(&monero_data).unwrap();
        let pow = ProofOfWork {
            accumulated_monero_difficulty: Default::default(),
            accumulated_blake_difficulty: Default::default(),
            target_difficulty: Default::default(),
            pow_algo: PowAlgorithm::Monero,
            pow_data: serialized,
        };
        block_header.pow = pow;
        assert_ne!(monero_difficulty(&block_header).as_u64(), 0);
    }
}

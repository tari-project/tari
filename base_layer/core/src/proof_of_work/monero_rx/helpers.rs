//  Copyright 2021, The Tari Project
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
use std::iter;

use log::*;
use monero::{
    blockdata::transaction::{ExtraField, SubField},
    consensus,
    cryptonote::hash::Hashable,
    VarInt,
};
use tari_utilities::hex::HexError;

use super::{
    error::MergeMineError,
    fixed_array::FixedByteArray,
    merkle_tree::{create_merkle_proof, tree_hash},
    pow_data::MoneroPowData,
};
use crate::{
    blocks::BlockHeader,
    proof_of_work::{
        randomx_factory::{RandomXFactory, RandomXVMInstance},
        Difficulty,
    },
};

pub const LOG_TARGET: &str = "c::pow::monero_rx";
///  Calculates the achieved Monero difficulty for the `BlockHeader`. An error is returned if the BlockHeader does not
/// contain valid Monero PoW data.
pub fn monero_difficulty(header: &BlockHeader, randomx_factory: &RandomXFactory) -> Result<Difficulty, MergeMineError> {
    let monero_pow_data = verify_header(header)?;
    debug!(target: LOG_TARGET, "Valid Monero data: {}", monero_pow_data);
    let blockhashing_blob = monero_pow_data.to_blockhashing_blob();
    let vm = randomx_factory.create(monero_pow_data.randomx_key())?;
    get_random_x_difficulty(&blockhashing_blob, &vm).map(|(diff, _)| diff)
}

fn get_random_x_difficulty(input: &[u8], vm: &RandomXVMInstance) -> Result<(Difficulty, Vec<u8>), MergeMineError> {
    let hash = vm.calculate_hash(input)?;
    debug!(target: LOG_TARGET, "RandomX Hash: {:?}", hash);
    let difficulty = Difficulty::little_endian_difficulty(&hash)?;
    Ok((difficulty, hash))
}

/// Validates the monero data contained in the given header, making these assetions:
/// 1. The MoneroPowData is well-formed (i.e. can be deserialized)
/// 1. The header's merge mining hash is included in the coinbase extra field
/// 1. The merkle proof and coinbase hash produce a matching merkle root
///
/// If these assertions pass, a valid `MoneroPowData` instance is returned
fn verify_header(header: &BlockHeader) -> Result<MoneroPowData, MergeMineError> {
    let monero_data = MoneroPowData::from_header(header)?;
    let expected_merge_mining_hash = header.mining_hash();
    let extra_field = ExtraField::try_parse(&monero_data.coinbase_tx.prefix.extra)
        .map_err(|_| MergeMineError::DeserializeError("Invalid extra field".to_string()))?;
    // Check that the Tari MM hash is found in the monero coinbase transaction
    let is_found = extra_field.0.iter().any(|item| match item {
        SubField::MergeMining(Some(depth), merge_mining_hash) => {
            depth == &VarInt(0) && merge_mining_hash.as_bytes() == expected_merge_mining_hash.as_slice()
        },
        _ => false,
    });

    if !is_found {
        return Err(MergeMineError::ValidationError(
            "Expected merge mining tag was not found in Monero coinbase transaction".to_string(),
        ));
    }

    if !monero_data.is_valid_merkle_root() {
        return Err(MergeMineError::InvalidMerkleRoot);
    }

    Ok(monero_data)
}

pub fn extract_tari_hash(monero: &monero::Block) -> Result<Option<monero::Hash>, MergeMineError> {
    let extra_field = ExtraField::try_parse(&monero.miner_tx.prefix.extra)
        .map_err(|_| MergeMineError::DeserializeError("Invalid extra field".to_string()))?;
    for item in &extra_field.0 {
        if let SubField::MergeMining(_depth, merge_mining_hash) = item {
            return Ok(Some(*merge_mining_hash));
        }
    }
    Ok(None)
}

pub fn deserialize_monero_block_from_hex<T>(data: T) -> Result<monero::Block, MergeMineError>
where T: AsRef<[u8]> {
    let bytes = hex::decode(data).map_err(|_| HexError::HexConversionError)?;
    let obj = consensus::deserialize::<monero::Block>(&bytes)
        .map_err(|_| MergeMineError::ValidationError("blocktemplate blob invalid".to_string()))?;
    Ok(obj)
}

pub fn serialize_monero_block_to_hex(obj: &monero::Block) -> Result<String, MergeMineError> {
    let data = consensus::serialize::<monero::Block>(obj);
    let bytes = hex::encode(data);
    Ok(bytes)
}

pub fn construct_monero_data(block: monero::Block, seed: FixedByteArray) -> Result<MoneroPowData, MergeMineError> {
    let hashes = create_ordered_transaction_hashes_from_block(&block);
    let root = tree_hash(&hashes)?;
    let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).ok_or_else(|| {
        MergeMineError::ValidationError(
            "create_merkle_proof returned None because the block had no coinbase (which is impossible because the \
             Block type does not allow that)"
                .to_string(),
        )
    })?;
    #[allow(clippy::cast_possible_truncation)]
    Ok(MoneroPowData {
        header: block.header,
        randomx_key: seed,
        transaction_count: hashes.len() as u16,
        merkle_root: root,
        coinbase_merkle_proof,
        coinbase_tx: block.miner_tx,
    })
}

/// Creates a hex encoded Monero blockhashing_blob that's used by the pow hash
pub fn create_blockhashing_blob_from_block(block: &monero::Block) -> Result<String, MergeMineError> {
    let tx_hashes = create_ordered_transaction_hashes_from_block(block);
    let root = tree_hash(&tx_hashes)?;
    let blob = create_block_hashing_blob(&block.header, &root, tx_hashes.len() as u64);
    Ok(hex::encode(blob))
}

pub fn create_ordered_transaction_hashes_from_block(block: &monero::Block) -> Vec<monero::Hash> {
    iter::once(block.miner_tx.hash())
        .chain(block.tx_hashes.clone())
        .collect()
}

/// Appends merge mining hash to a Monero block
pub fn append_merge_mining_tag<T: AsRef<[u8]>>(block: &mut monero::Block, hash: T) -> Result<(), MergeMineError> {
    if hash.as_ref().len() != monero::Hash::len_bytes() {
        return Err(MergeMineError::HashingError(format!(
            "Expected source to be {} bytes, but it was {} bytes",
            monero::Hash::len_bytes(),
            hash.as_ref().len()
        )));
    }
    let hash = monero::Hash::from_slice(hash.as_ref());
    let mm_tag = SubField::MergeMining(Some(VarInt(0)), hash);
    let mut extra_field = ExtraField::try_parse(&block.miner_tx.prefix.extra)
        .map_err(|_| MergeMineError::DeserializeError("Invalid extra field".to_string()))?;
    extra_field.0.push(mm_tag);
    block.miner_tx.prefix.extra = extra_field.into();
    Ok(())
}

/// Creates a hex encoded Monero blockhashing_blob
pub fn create_block_hashing_blob(
    header: &monero::BlockHeader,
    merkle_root: &monero::Hash,
    transaction_count: u64,
) -> Vec<u8> {
    let mut blockhashing_blob = consensus::serialize(header);
    blockhashing_blob.extend_from_slice(merkle_root.as_bytes());
    let mut count = consensus::serialize(&VarInt(transaction_count));
    blockhashing_blob.append(&mut count);
    blockhashing_blob
}

#[cfg(test)]
mod test {
    use std::convert::{TryFrom, TryInto};

    use borsh::BorshSerialize;
    use monero::{
        blockdata::transaction::{ExtraField, TxOutTarget},
        consensus::deserialize,
        cryptonote::hash::Hashable,
        util::ringct::{RctSig, RctSigBase, RctType},
        Hash,
        PublicKey,
        Transaction,
        TransactionPrefix,
        TxIn,
        TxOut,
    };
    use tari_common_types::types::FixedHash;
    use tari_test_utils::unpack_enum;
    use tari_utilities::{
        epoch_time::EpochTime,
        hex::{from_hex, Hex},
        ByteArray,
    };

    use super::*;
    use crate::proof_of_work::{monero_rx::fixed_array::FixedByteArray, PowAlgorithm, ProofOfWork};

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
                        key: hex::decode("e2e19d8badb15e77c8e1f441cf6acd9bcde34a07cae82bbe5ff9629bf88e6e81")
                            .unwrap()
                            .as_slice()
                            .try_into()
                            .unwrap(),
                    },
                }],
                extra: ExtraField(vec![
                    SubField::TxPublicKey(PublicKey::from_slice(pk_extra.as_slice()).unwrap()),
                    SubField::Nonce(vec![196, 37, 4, 0, 27, 37, 187, 163, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
                ])
                .into(),
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
        let hex = hex::encode(consensus::serialize::<Transaction>(&transaction));
        deserialize::<Transaction>(&hex::decode(hex).unwrap()).unwrap();
    }

    // This tests checks the blockhashing blob of monero-rs
    #[test]
    fn test_monero_rs_block_serialize() {
        // block with only the miner tx and no other transactions
        let hex = "0c0c94debaf805beb3489c722a285c092a32e7c6893abfc7d069699c8326fc3445a749c5276b6200000000029b892201ffdf882201b699d4c8b1ec020223df524af2a2ef5f870adb6e1ceb03a475c39f8b9ef76aa50b46ddd2a18349402b012839bfa19b7524ec7488917714c216ca254b38ed0424ca65ae828a7c006aeaf10208f5316a7f6b99cca60000";
        // blockhashing blob for above block as accepted by monero
        let hex_blockhash_blob="0c0c94debaf805beb3489c722a285c092a32e7c6893abfc7d069699c8326fc3445a749c5276b6200000000602d0d4710e2c2d38da0cce097accdf5dc18b1d34323880c1aae90ab8f6be6e201";
        let bytes = hex::decode(hex).unwrap();
        let block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let header = consensus::serialize::<monero::BlockHeader>(&block.header);
        let tx_count = 1 + block.tx_hashes.len() as u64;
        let mut count = consensus::serialize::<VarInt>(&VarInt(tx_count));
        #[allow(clippy::cast_possible_truncation)]
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
        let bytes2 = consensus::serialize::<monero::Block>(&block);
        assert_eq!(bytes, bytes2);
        let hex2 = hex::encode(bytes2);
        assert_eq!(hex, hex2);
    }

    #[test]
    fn test_monero_data() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            output_mmr_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
        };
        let hash = block_header.mining_hash();
        append_merge_mining_tag(&mut block, hash).unwrap();
        let hashes = create_ordered_transaction_hashes_from_block(&block);
        assert_eq!(hashes.len(), block.tx_hashes.len() + 1);
        let root = tree_hash(&hashes).unwrap();
        let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();

        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: u16::try_from(hashes.len()).unwrap(),
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_tx: block.miner_tx,
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::Monero,
            pow_data: serialized,
        };
        block_header.pow = pow;
        MoneroPowData::from_header(&block_header).unwrap();
    }

    #[test]
    fn test_input_blob() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let input_blob = create_blockhashing_blob_from_block(&block).unwrap();
        assert_eq!(input_blob, "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000058b030b6800d433bbcb2b560afe2a08e4dc152fa77ead96d37aaf14897d3c09601");
    }

    #[test]
    fn test_append_mm_tag() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            output_mmr_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
        };
        let hash = block_header.mining_hash();
        append_merge_mining_tag(&mut block, hash).unwrap();
        let count = 1 + (u16::try_from(block.tx_hashes.len()).unwrap());
        let mut hashes = Vec::with_capacity(count as usize);
        hashes.push(block.miner_tx.hash());
        // Note: tx_hashes is empty, so |hashes| == 1
        for item in block.clone().tx_hashes {
            hashes.push(item);
        }
        let root = tree_hash(&hashes).unwrap();
        assert_eq!(root, hashes[0]);
        let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();
        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: count,
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_tx: block.miner_tx,
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::Monero,
            pow_data: serialized,
        };
        block_header.pow = pow;
        verify_header(&block_header).unwrap();
    }

    #[test]
    fn test_append_mm_tag_no_tag() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            output_mmr_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
        };
        let count = 1 + (u16::try_from(block.tx_hashes.len()).unwrap());
        let mut hashes = Vec::with_capacity(count as usize);
        hashes.push(block.miner_tx.hash());
        for item in block.clone().tx_hashes {
            hashes.push(item);
        }
        let root = tree_hash(&hashes).unwrap();
        let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();
        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: count,
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_tx: block.miner_tx,
        };

        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::Monero,
            pow_data: serialized,
        };
        block_header.pow = pow;
        let err = verify_header(&block_header).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("Expected merge mining tag was not found in Monero coinbase transaction"));
    }

    #[test]
    fn test_append_mm_tag_wrong_hash() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            output_mmr_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
        };
        let hash = Hash::null();
        append_merge_mining_tag(&mut block, hash).unwrap();
        let count = 1 + (u16::try_from(block.tx_hashes.len()).unwrap());
        let mut hashes = Vec::with_capacity(count as usize);
        let mut proof = Vec::with_capacity(count as usize);
        hashes.push(block.miner_tx.hash());
        proof.push(block.miner_tx.hash());
        for item in block.clone().tx_hashes {
            hashes.push(item);
            proof.push(item);
        }
        let root = tree_hash(&hashes).unwrap();
        let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();
        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: count,
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_tx: block.miner_tx,
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::Monero,
            pow_data: serialized,
        };
        block_header.pow = pow;
        let err = verify_header(&block_header).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("Expected merge mining tag was not found in Monero coinbase transaction"));
    }

    #[test]
    fn test_verify_header_no_coinbase() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            output_mmr_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
        };
        let hash = block_header.mining_hash();
        append_merge_mining_tag(&mut block, hash).unwrap();
        let count = 1 + (u16::try_from(block.tx_hashes.len()).unwrap());
        let mut hashes = Vec::with_capacity(count as usize);
        let mut proof = Vec::with_capacity(count as usize);
        hashes.push(block.miner_tx.hash());
        proof.push(block.miner_tx.hash());
        for item in block.clone().tx_hashes {
            hashes.push(item);
            proof.push(item);
        }
        let root = tree_hash(&hashes).unwrap();
        let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();
        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: count,
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_tx: Default::default(),
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::Monero,
            pow_data: serialized,
        };
        block_header.pow = pow;
        let err = verify_header(&block_header).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("Expected merge mining tag was not found in Monero coinbase transaction"));
    }

    #[test]
    fn test_verify_header_no_data() {
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            output_mmr_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
        };
        let monero_data = MoneroPowData {
            header: Default::default(),
            randomx_key: FixedByteArray::default(),
            transaction_count: 1,
            merkle_root: Default::default(),
            coinbase_merkle_proof: create_merkle_proof(&[Hash::null()], &Hash::null()).unwrap(),
            coinbase_tx: Default::default(),
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::Monero,
            pow_data: serialized,
        };
        block_header.pow = pow;
        let err = verify_header(&block_header).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("Expected merge mining tag was not found in Monero coinbase transaction"));
    }

    #[test]
    fn test_verify_invalid_root() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            output_mmr_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
        };
        let hash = block_header.mining_hash();
        append_merge_mining_tag(&mut block, hash).unwrap();
        let count = 1 + (u16::try_from(block.tx_hashes.len()).unwrap());
        let mut hashes = Vec::with_capacity(count as usize);
        let mut proof = Vec::with_capacity(count as usize);
        hashes.push(block.miner_tx.hash());
        proof.push(block.miner_tx.hash());
        for item in block.clone().tx_hashes {
            hashes.push(item);
            proof.push(item);
        }

        let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();
        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: count,
            merkle_root: Hash::null(),
            coinbase_merkle_proof,
            coinbase_tx: block.miner_tx,
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::Monero,
            pow_data: serialized,
        };
        block_header.pow = pow;
        let err = verify_header(&block_header).unwrap_err();
        unpack_enum!(MergeMineError::InvalidMerkleRoot = err);
    }

    #[test]
    fn test_difficulty() {
        // Taken from block: https://stagenet.xmrchain.net/search?value=672576
        let versions = "0c0c";
        // Tool for encoding VarInts:
        // https://gchq.github.io/CyberChef/#recipe=VarInt_Encode()To_Hex('Space',0)From_Hex('Auto'/disabled)VarInt_Decode(/disabled)&input=MTYwMTAzMTIwMg
        let timestamp = "a298b7fb05"; // 1601031202
        let prev_block = "046f4fe371f9acdc27c377f4adee84e93b11f89246a74dd77f1bf0856141da5c";
        let nonce = "FE394F12"; // 307182078
        let tx_hash = "77139305ea53cfe95cf7235d2fed6fca477395b019b98060acdbc0f8fb0b8b92"; // miner tx
        let count = "01";

        let input = from_hex(&format!(
            "{}{}{}{}{}{}",
            versions, timestamp, prev_block, nonce, tx_hash, count
        ))
        .unwrap();
        let key = from_hex("2aca6501719a5c7ab7d4acbc7cc5d277b57ad8c27c6830788c2d5a596308e5b1").unwrap();
        let rx = RandomXFactory::default();

        let (difficulty, hash) = get_random_x_difficulty(&input, &rx.create(&key).unwrap()).unwrap();
        assert_eq!(
            hash.to_hex(),
            "f68fbc8cc85bde856cd1323e9f8e6f024483038d728835de2f8c014ff6260000"
        );
        assert_eq!(difficulty.as_u64(), 430603);
    }
}

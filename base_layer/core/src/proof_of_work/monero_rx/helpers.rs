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
    blockdata::transaction::{ExtraField, RawExtraField, SubField},
    consensus,
    consensus::Encodable,
    cryptonote::hash::Hashable,
    VarInt,
};
use primitive_types::U256;
use sha2::{Digest, Sha256};
use tari_common_types::types::FixedHash;
use tari_utilities::hex::HexError;
use tiny_keccak::{Hasher, Keccak};

use super::{
    error::MergeMineError,
    fixed_array::FixedByteArray,
    merkle_tree::{create_merkle_proof, tree_hash},
    pow_data::MoneroPowData,
};
use crate::{
    blocks::BlockHeader,
    consensus::ConsensusManager,
    proof_of_work::{
        monero_rx::merkle_tree_parameters::MerkleTreeParameters,
        randomx_factory::{RandomXFactory, RandomXVMInstance},
        Difficulty,
    },
};

pub const LOG_TARGET: &str = "c::pow::monero_rx";
///  Calculates the achieved Monero difficulty for the `BlockHeader`. An error is returned if the BlockHeader does not
/// contain valid Monero PoW data.
pub fn randomx_difficulty(
    header: &BlockHeader,
    randomx_factory: &RandomXFactory,
    genesis_block_hash: &FixedHash,
    consensus: &ConsensusManager,
) -> Result<Difficulty, MergeMineError> {
    let monero_pow_data = verify_header(header, genesis_block_hash, consensus)?;
    debug!(target: LOG_TARGET, "Valid Monero data: {}", monero_pow_data);
    let blockhashing_blob = monero_pow_data.to_blockhashing_blob();
    let vm = randomx_factory.create(monero_pow_data.randomx_key())?;
    get_random_x_difficulty(&blockhashing_blob, &vm).map(|(diff, _)| diff)
}

/// Calculate the RandomX mining hash using the virtual machine together with the achieved difficulty
fn get_random_x_difficulty(input: &[u8], vm: &RandomXVMInstance) -> Result<(Difficulty, Vec<u8>), MergeMineError> {
    let hash = vm.calculate_hash(input)?;
    debug!(target: LOG_TARGET, "RandomX Hash: {:?}", hash);
    let difficulty = Difficulty::little_endian_difficulty(&hash)?;
    Ok((difficulty, hash))
}

// Parsing an extra field from bytes will always return an extra field with sub-fields that could be read, even if it
// does not represent the original extra field. As per Monero consensus rules, an error here will not represent a
// failure to deserialize a block, so no need to error here.
fn parse_extra_field_truncate_on_error(raw_extra_field: &RawExtraField) -> ExtraField {
    match ExtraField::try_parse(raw_extra_field) {
        Ok(val) => val,
        Err(val) => {
            warn!(
                target: LOG_TARGET,
                "Some sub-fields could not be parsed successfully from the Monero coinbase extra field and will be \
                excluded"
            );
            val
        },
    }
}

/// Validates the monero data contained in the given header, making these assetions:
/// 1. The MoneroPowData is well-formed (i.e. can be deserialized)
/// 1. The header's merge mining hash is included in the coinbase extra field
/// 1. The merkle proof and coinbase hash produce a matching merkle root
///
/// If these assertions pass, a valid `MoneroPowData` instance is returned
pub fn verify_header(
    header: &BlockHeader,
    genesis_block_hash: &FixedHash,
    consensus: &ConsensusManager,
) -> Result<MoneroPowData, MergeMineError> {
    let monero_data = MoneroPowData::from_header(header, consensus)?;
    let expected_merge_mining_hash = header.merge_mining_hash();
    let extra_field = ExtraField::try_parse(&monero_data.coinbase_tx_extra);
    let extra_field = extra_field.unwrap_or_else(|ex_field| {
        warn!(target: LOG_TARGET, "Error deserializing, Monero extra field");
        ex_field
    });
    debug!(target: LOG_TARGET, "Extra field: {:?}", extra_field);
    // Check that the Tari MM hash is found in the Monero coinbase transaction
    // and that only 1 Tari header is found

    let mut is_found = false;
    let mut already_seen_mmfield = false;
    for item in extra_field.0 {
        if let SubField::MergeMining(Some(depth), merge_mining_hash) = item {
            if already_seen_mmfield {
                return Err(MergeMineError::ValidationError(
                    "More than one merge mining tag found in coinbase".to_string(),
                ));
            }
            already_seen_mmfield = true;
            is_found = check_aux_chains(
                &monero_data,
                depth,
                &merge_mining_hash,
                &expected_merge_mining_hash,
                genesis_block_hash,
            )
        }
    }

    if !is_found {
        return Err(MergeMineError::ValidationError(
            "Expected merge mining tag was not found in Monero coinbase transaction".to_string(),
        ));
    }

    if !monero_data.is_coinbase_valid_merkle_root() {
        return Err(MergeMineError::InvalidMerkleRoot);
    }

    Ok(monero_data)
}

fn check_aux_chains(
    monero_data: &MoneroPowData,
    merge_mining_params: VarInt,
    aux_chain_merkle_root: &monero::Hash,
    tari_hash: &FixedHash,
    tari_genesis_block_hash: &FixedHash,
) -> bool {
    let t_hash = monero::Hash::from_slice(tari_hash.as_slice());
    if merge_mining_params == VarInt(0) {
        // we interpret 0 as there is only 1 chain, tari.
        if t_hash == *aux_chain_merkle_root {
            return true;
        }
    }
    let merkle_tree_params = MerkleTreeParameters::from_varint(merge_mining_params);
    if merkle_tree_params.number_of_chains == 0 {
        return false;
    }
    let hash_position = U256::from_little_endian(
        &Sha256::new()
            .chain_update(tari_genesis_block_hash)
            .chain_update(merkle_tree_params.aux_nonce.to_le_bytes())
            .chain_update((109_u8).to_le_bytes())
            .finalize(),
    )
    .low_u32() %
        u32::from(merkle_tree_params.number_of_chains);
    let (merkle_root, pos) = monero_data.aux_chain_merkle_proof.calculate_root_with_pos(&t_hash);
    if hash_position != pos {
        return false;
    }

    merkle_root == *aux_chain_merkle_root
}

/// Extracts the Monero block hash from the coinbase transaction's extra field
pub fn extract_aux_merkle_root_from_block(monero: &monero::Block) -> Result<Option<monero::Hash>, MergeMineError> {
    // When we extract the merge mining hash, we do not care if the extra field can be parsed without error.
    let extra_field = parse_extra_field_truncate_on_error(&monero.miner_tx.prefix.extra);

    // Only one merge mining tag is allowed
    let merge_mining_hashes: Vec<monero::Hash> = extra_field
        .0
        .iter()
        .filter_map(|item| {
            if let SubField::MergeMining(_depth, merge_mining_hash) = item {
                Some(*merge_mining_hash)
            } else {
                None
            }
        })
        .collect();
    if merge_mining_hashes.len() > 1 {
        return Err(MergeMineError::ValidationError(
            "More than one merge mining tag found in coinbase".to_string(),
        ));
    }

    if let Some(merge_mining_hash) = merge_mining_hashes.into_iter().next() {
        Ok(Some(merge_mining_hash))
    } else {
        Ok(None)
    }
}

/// Deserializes the given hex-encoded string into a Monero block
pub fn deserialize_monero_block_from_hex<T>(data: T) -> Result<monero::Block, MergeMineError>
where T: AsRef<[u8]> {
    let bytes = hex::decode(data).map_err(|_| HexError::HexConversionError {})?;
    let obj = consensus::deserialize::<monero::Block>(&bytes)
        .map_err(|_| MergeMineError::ValidationError("blocktemplate blob invalid".to_string()))?;
    Ok(obj)
}

/// Serializes the given Monero block into a hex-encoded string
pub fn serialize_monero_block_to_hex(obj: &monero::Block) -> Result<String, MergeMineError> {
    let data = consensus::serialize::<monero::Block>(obj);
    let bytes = hex::encode(data);
    Ok(bytes)
}

/// Constructs the Monero PoW data from the given block and seed
pub fn construct_monero_data(
    block: monero::Block,
    seed: FixedByteArray,
    ordered_aux_chain_hashes: Vec<monero::Hash>,
    tari_hash: FixedHash,
) -> Result<MoneroPowData, MergeMineError> {
    let hashes = create_ordered_transaction_hashes_from_block(&block);
    let root = tree_hash(&hashes)?;
    let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).ok_or_else(|| {
        MergeMineError::ValidationError(
            "create_merkle_proof returned None because the block had no coinbase (which is impossible because the \
             Block type does not allow that)"
                .to_string(),
        )
    })?;
    let coinbase = block.miner_tx.clone();

    let mut keccak = Keccak::v256();
    let mut encoder_prefix = Vec::new();
    coinbase
        .prefix
        .version
        .consensus_encode(&mut encoder_prefix)
        .map_err(|e| MergeMineError::SerializeError(e.to_string()))?;
    coinbase
        .prefix
        .unlock_time
        .consensus_encode(&mut encoder_prefix)
        .map_err(|e| MergeMineError::SerializeError(e.to_string()))?;
    coinbase
        .prefix
        .inputs
        .consensus_encode(&mut encoder_prefix)
        .map_err(|e| MergeMineError::SerializeError(e.to_string()))?;
    coinbase
        .prefix
        .outputs
        .consensus_encode(&mut encoder_prefix)
        .map_err(|e| MergeMineError::SerializeError(e.to_string()))?;
    keccak.update(&encoder_prefix);

    let t_hash = monero::Hash::from_slice(tari_hash.as_slice());
    let aux_chain_merkle_proof = create_merkle_proof(&ordered_aux_chain_hashes, &t_hash).ok_or_else(|| {
        MergeMineError::ValidationError(
            "create_merkle_proof returned None, could not find tari hash in ordered aux chain hashes".to_string(),
        )
    })?;
    #[allow(clippy::cast_possible_truncation)]
    Ok(MoneroPowData {
        header: block.header,
        randomx_key: seed,
        transaction_count: hashes.len() as u16,
        merkle_root: root,
        coinbase_merkle_proof,
        coinbase_tx_extra: block.miner_tx.prefix.extra,
        coinbase_tx_hasher: keccak,
        aux_chain_merkle_proof,
    })
}

/// Creates a hex encoded Monero blockhashing_blob that's used by the pow hash
pub fn create_blockhashing_blob_from_block(block: &monero::Block) -> Result<String, MergeMineError> {
    let tx_hashes = create_ordered_transaction_hashes_from_block(block);
    let root = tree_hash(&tx_hashes)?;
    let blob = create_block_hashing_blob(&block.header, &root, tx_hashes.len() as u64);
    Ok(hex::encode(blob))
}

/// Create a set of ordered transaction hashes from a Monero block
pub fn create_ordered_transaction_hashes_from_block(block: &monero::Block) -> Vec<monero::Hash> {
    iter::once(block.miner_tx.hash())
        .chain(block.tx_hashes.clone())
        .collect()
}

/// Inserts aux chain merkle root and info into a Monero block
pub fn insert_aux_chain_mr_and_info_into_block<T: AsRef<[u8]>>(
    block: &mut monero::Block,
    aux_chain_mr: T,
    aux_chain_count: u8,
    aux_nonce: u32,
) -> Result<(), MergeMineError> {
    if aux_chain_count == 0 {
        return Err(MergeMineError::ZeroAuxChains);
    }
    if aux_chain_count == 0 {
        return Err(MergeMineError::ZeroAuxChains);
    }
    if aux_chain_mr.as_ref().len() != monero::Hash::len_bytes() {
        return Err(MergeMineError::HashingError(format!(
            "Expected source to be {} bytes, but it was {} bytes",
            monero::Hash::len_bytes(),
            aux_chain_mr.as_ref().len()
        )));
    }
    // When we insert the merge mining tag, we need to make sure that the extra field is valid.
    let mut extra_field = ExtraField::try_parse(&block.miner_tx.prefix.extra)
        .map_err(|_| MergeMineError::DeserializeError("Invalid extra field".to_string()))?;

    // Adding more than one merge mining tag is not allowed
    for item in &extra_field.0 {
        if let SubField::MergeMining(Some(_), _) = item {
            return Err(MergeMineError::ValidationError(
                "More than one merge mining tag in coinbase not allowed".to_string(),
            ));
        }
    }

    // If `SubField::Padding(n)` with `n < 255` is the last sub field in the extra field, then appending a new field
    // will always fail deserialization (`ExtraField::try_parse`) - the new field cannot be parsed in that sequence.
    // To circumvent this, we create a new extra field by appending the original extra field to the merge mining field
    // instead.
    let hash = monero::Hash::from_slice(aux_chain_mr.as_ref());
    let encoded = if aux_chain_count == 1 {
        VarInt(0)
    } else {
        let mt_params = MerkleTreeParameters {
            number_of_chains: aux_chain_count,
            aux_nonce,
        };
        mt_params.to_varint()
    };
    let mt_params = MerkleTreeParameters {
        number_of_chains: aux_chain_count,
        aux_nonce,
    };
    let encoded = if aux_chain_count == 1 {
        VarInt(0)
    } else {
        mt_params.to_varint()
    };
    extra_field.0.insert(0, SubField::MergeMining(Some(encoded), hash));
    debug!(target: LOG_TARGET, "Inserted extra field: {:?}", extra_field);

    block.miner_tx.prefix.extra = extra_field.into();

    // lets test the block to ensure its serializes correctly
    let blocktemplate_blob = serialize_monero_block_to_hex(block)?;
    let bytes = hex::decode(blocktemplate_blob).map_err(|_| HexError::HexConversionError {})?;
    let de_block = monero::consensus::deserialize::<monero::Block>(&bytes[..])
        .map_err(|_| MergeMineError::ValidationError("blocktemplate blob invalid".to_string()))?;
    if block != &de_block {
        return Err(MergeMineError::SerializeError(
            "Blocks dont match after serialization".to_string(),
        ));
    }
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
    use tari_common::configuration::Network;
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
    fn test_monero_partial_hash() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let block_header = BlockHeader::new(0);
        let hash = block_header.merge_mining_hash();
        insert_aux_chain_mr_and_info_into_block(&mut block, hash, 1, 0).unwrap();

        let coinbase = block.miner_tx.clone();
        let extra = coinbase.prefix.extra;
        let mut keccak = Keccak::v256();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
        keccak.update(&encoder_prefix);

        let mut finalised_prefix_keccak = keccak.clone();
        let mut encoder_extra_field = Vec::new();
        extra.consensus_encode(&mut encoder_extra_field).unwrap();
        finalised_prefix_keccak.update(&encoder_extra_field);
        let mut prefix_hash: [u8; 32] = [0; 32];
        finalised_prefix_keccak.finalize(&mut prefix_hash);

        let test_prefix_hash = block.miner_tx.prefix.hash();
        let test2 = monero::Hash::from_slice(&prefix_hash);
        assert_eq!(test_prefix_hash, test2);

        // let mut finalised_keccak = Keccak::v256();
        let rct_sig_base = RctSigBase {
            rct_type: RctType::Null,
            txn_fee: Default::default(),
            pseudo_outs: vec![],
            ecdh_info: vec![],
            out_pk: vec![],
        };
        let hashes = vec![test2, rct_sig_base.hash(), monero::Hash::null()];
        let encoder_final: Vec<u8> = hashes.into_iter().flat_map(|h| Vec::from(&h.to_bytes()[..])).collect();
        let coinbase = monero::Hash::new(encoder_final);
        let coinbase_hash = block.miner_tx.hash();
        assert_eq!(coinbase, coinbase_hash);
    }

    #[test]
    fn test_monero_data() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let rules = ConsensusManager::builder(Network::LocalNet).build().unwrap();
        let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 0,
        };
        let hash = block_header.merge_mining_hash();
        insert_aux_chain_mr_and_info_into_block(&mut block, hash, 1, 0).unwrap();
        let hashes = create_ordered_transaction_hashes_from_block(&block);
        assert_eq!(hashes.len(), block.tx_hashes.len() + 1);
        let root = tree_hash(&hashes).unwrap();
        let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();
        let aux_hashes = vec![monero::Hash::from_slice(hash.as_ref())];
        let aux_chain_merkle_proof = create_merkle_proof(&aux_hashes, &aux_hashes[0]).unwrap();

        let coinbase = block.miner_tx.clone();
        let extra = coinbase.prefix.extra;
        let mut keccak = Keccak::v256();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
        keccak.update(&encoder_prefix);

        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_canonical_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: u16::try_from(hashes.len()).unwrap(),
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_tx_hasher: keccak.clone(),
            coinbase_tx_extra: extra.clone(),
            aux_chain_merkle_proof,
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomX,
            pow_data: serialized,
        };
        block_header.pow = pow;
        MoneroPowData::from_header(&block_header, &rules).unwrap();

        // lets test the hashesh
        let mut finalised_prefix_keccak = keccak.clone();
        let mut encoder_extra_field = Vec::new();
        extra.consensus_encode(&mut encoder_extra_field).unwrap();
        finalised_prefix_keccak.update(&encoder_extra_field);
        let mut prefix_hash: [u8; 32] = [0; 32];
        finalised_prefix_keccak.finalize(&mut prefix_hash);

        let test_prefix_hash = block.miner_tx.prefix.hash();
        let test2 = monero::Hash::from_slice(&prefix_hash);
        assert_eq!(test_prefix_hash, test2);

        // let mut finalised_keccak = Keccak::v256();
        let rct_sig_base = RctSigBase {
            rct_type: RctType::Null,
            txn_fee: Default::default(),
            pseudo_outs: vec![],
            ecdh_info: vec![],
            out_pk: vec![],
        };
        let hashes = vec![test2, rct_sig_base.hash(), monero::Hash::null()];
        let encoder_final: Vec<u8> = hashes.into_iter().flat_map(|h| Vec::from(&h.to_bytes()[..])).collect();
        let coinbase = monero::Hash::new(encoder_final);
        let coinbase_hash = block.miner_tx.hash();
        assert_eq!(coinbase, coinbase_hash);
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
        let rules = ConsensusManager::builder(Network::LocalNet).build().unwrap();
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
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 0,
        };
        let hash = block_header.merge_mining_hash();
        insert_aux_chain_mr_and_info_into_block(&mut block, hash, 1, 0).unwrap();
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
        let aux_hashes = vec![monero::Hash::from_slice(hash.as_ref())];
        let aux_chain_merkle_proof = create_merkle_proof(&aux_hashes, &aux_hashes[0]).unwrap();

        let coinbase = block.miner_tx.clone();
        let extra = coinbase.prefix.extra.clone();
        let mut keccak = Keccak::v256();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
        keccak.update(&encoder_prefix);

        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_canonical_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: count,
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_tx_hasher: keccak,
            coinbase_tx_extra: extra,
            aux_chain_merkle_proof,
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomX,
            pow_data: serialized,
        };
        block_header.pow = pow;

        verify_header(&block_header, &hash, &rules).unwrap();
    }

    #[test]
    fn test_append_mm_tag_no_tag() {
        let rules = ConsensusManager::builder(Network::LocalNet).build().unwrap();
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
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 0,
        };
        let count = 1 + (u16::try_from(block.tx_hashes.len()).unwrap());
        let mut hashes = Vec::with_capacity(count as usize);
        hashes.push(block.miner_tx.hash());
        for item in block.clone().tx_hashes {
            hashes.push(item);
        }
        let root = tree_hash(&hashes).unwrap();
        let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();
        let aux_hashes = vec![monero::Hash::from_slice(block_header.hash().as_ref())];
        let aux_chain_merkle_proof = create_merkle_proof(&aux_hashes, &aux_hashes[0]).unwrap();

        let coinbase = block.miner_tx.clone();
        let extra = coinbase.prefix.extra.clone();
        let mut keccak = Keccak::v256();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
        keccak.update(&encoder_prefix);

        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_canonical_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: count,
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_tx_hasher: keccak,
            coinbase_tx_extra: extra,
            aux_chain_merkle_proof,
        };

        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomX,
            pow_data: serialized,
        };
        block_header.pow = pow;
        let err = verify_header(&block_header, &block_header.hash(), &rules).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("Expected merge mining tag was not found in Monero coinbase transaction"));
    }

    #[test]
    fn test_append_mm_tag_wrong_hash() {
        let rules = ConsensusManager::builder(Network::LocalNet).build().unwrap();
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
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 0,
        };
        let hash = Hash::null();
        insert_aux_chain_mr_and_info_into_block(&mut block, hash, 1, 0).unwrap();
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
        let aux_hashes = vec![monero::Hash::from_slice(hash.as_ref())];
        let aux_chain_merkle_proof = create_merkle_proof(&aux_hashes, &aux_hashes[0]).unwrap();

        let coinbase = block.miner_tx.clone();
        let extra = coinbase.prefix.extra.clone();
        let mut keccak = Keccak::v256();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
        keccak.update(&encoder_prefix);

        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_canonical_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: count,
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_tx_hasher: keccak,
            coinbase_tx_extra: extra,
            aux_chain_merkle_proof,
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomX,
            pow_data: serialized,
        };
        block_header.pow = pow;
        let err = verify_header(&block_header, &block_header.hash(), &rules).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("Expected merge mining tag was not found in Monero coinbase transaction"));
    }

    #[test]
    fn test_duplicate_append_mm_tag() {
        let rules = ConsensusManager::builder(Network::LocalNet).build().unwrap();
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
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 0,
        };
        let hash = block_header.merge_mining_hash();
        insert_aux_chain_mr_and_info_into_block(&mut block, hash, 1, 0).unwrap();
        #[allow(clippy::redundant_clone)]
        let mut block_header2 = block_header.clone();
        block_header2.version = 1;
        let hash2 = block_header2.merge_mining_hash();
        assert!(extract_aux_merkle_root_from_block(&block).is_ok());

        // Try via the API - this will fail because more than one merge mining tag is not allowed
        assert!(insert_aux_chain_mr_and_info_into_block(&mut block, hash2, 1, 0).is_err());

        // Now bypass the API - this will effectively allow us to insert more than one merge mining tag,
        // like trying to sneek it in. Later on, when we call `verify_header(&block_header)`, it should fail.
        let mut extra_field = ExtraField::try_parse(&block.miner_tx.prefix.extra).unwrap();
        let hash = monero::Hash::from_slice(hash.as_ref());
        extra_field.0.insert(0, SubField::MergeMining(Some(VarInt(0)), hash));
        block.miner_tx.prefix.extra = extra_field.into();

        // Trying to extract the Tari hash will fail because there are more than one merge mining tag
        let err = extract_aux_merkle_root_from_block(&block).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("More than one merge mining tag found in coinbase"));

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
        let aux_hashes = vec![monero::Hash::from_slice(hash.as_ref())];
        let aux_chain_merkle_proof = create_merkle_proof(&aux_hashes, &aux_hashes[0]).unwrap();

        let coinbase = block.miner_tx.clone();
        let extra = coinbase.prefix.extra.clone();
        let mut keccak = Keccak::v256();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
        keccak.update(&encoder_prefix);

        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_canonical_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: count,
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_tx_hasher: keccak,
            coinbase_tx_extra: extra,
            aux_chain_merkle_proof,
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomX,
            pow_data: serialized,
        };
        block_header.pow = pow;

        // Header verification will fail because there are more than one merge mining tag
        let err = verify_header(&block_header, &block_header.hash(), &rules).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("More than one merge mining tag found in coinbase"));
    }

    #[test]
    fn test_extra_field_with_parsing_error() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 2,
        };

        // Let us manipulate the extra field to make it invalid
        let mut extra_field_before_parse = ExtraField::try_parse(&block.miner_tx.prefix.extra).unwrap();
        assert_eq!(
            "ExtraField([TxPublicKey(06225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa4782), Nonce([246, \
             58, 168, 109, 46, 133, 127, 7])])",
            &format!("{:?}", extra_field_before_parse)
        );
        assert!(ExtraField::try_parse(&extra_field_before_parse.clone().into()).is_ok());

        extra_field_before_parse.0.insert(0, SubField::Padding(230));
        assert_eq!(
            "ExtraField([Padding(230), TxPublicKey(06225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa4782), \
             Nonce([246, 58, 168, 109, 46, 133, 127, 7])])",
            &format!("{:?}", extra_field_before_parse)
        );
        assert!(ExtraField::try_parse(&extra_field_before_parse.clone().into()).is_err());

        // Now insert the merge mining tag - this would also clean up the extra field and remove the invalid sub-fields
        let hash = block_header.merge_mining_hash();
        insert_aux_chain_mr_and_info_into_block(&mut block, hash, 1, 0).unwrap();
        assert!(ExtraField::try_parse(&block.miner_tx.prefix.extra.clone()).is_ok());

        // Verify that the merge mining tag is there
        let extra_field_after_tag = ExtraField::try_parse(&block.miner_tx.prefix.extra.clone()).unwrap();
        assert_eq!(
            &format!(
                "ExtraField([MergeMining(Some(0), 0x{}), \
                 TxPublicKey(06225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa4782), Nonce([246, 58, 168, \
                 109, 46, 133, 127, 7])])",
                hex::encode(hash)
            ),
            &format!("{:?}", extra_field_after_tag)
        );
    }

    #[test]
    fn test_verify_header_no_coinbase() {
        let rules = ConsensusManager::builder(Network::LocalNet).build().unwrap();
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
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 0,
        };
        let hash = block_header.merge_mining_hash();
        insert_aux_chain_mr_and_info_into_block(&mut block, hash, 1, 0).unwrap();
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
        let aux_hashes = vec![monero::Hash::from_slice(hash.as_ref())];
        let aux_chain_merkle_proof = create_merkle_proof(&aux_hashes, &aux_hashes[0]).unwrap();

        let coinbase: monero::Transaction = Default::default();
        let extra = coinbase.prefix.extra.clone();
        let mut keccak = Keccak::v256();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
        keccak.update(&encoder_prefix);

        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_canonical_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: count,
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_tx_hasher: keccak,
            coinbase_tx_extra: extra,
            aux_chain_merkle_proof,
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomX,
            pow_data: serialized,
        };
        block_header.pow = pow;
        let err = verify_header(&block_header, &block_header.hash(), &rules).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("Expected merge mining tag was not found in Monero coinbase transaction"));
    }

    #[test]
    fn test_verify_header_no_data() {
        let rules = ConsensusManager::builder(Network::LocalNet).build().unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 0,
        };
        let coinbase: monero::Transaction = Default::default();

        let extra = coinbase.prefix.extra.clone();
        let mut keccak = Keccak::v256();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
        keccak.update(&encoder_prefix);

        let monero_data = MoneroPowData {
            header: Default::default(),
            randomx_key: FixedByteArray::default(),
            transaction_count: 1,
            merkle_root: Default::default(),
            coinbase_merkle_proof: create_merkle_proof(&[Hash::null()], &Hash::null()).unwrap(),
            coinbase_tx_hasher: keccak,
            coinbase_tx_extra: extra,
            aux_chain_merkle_proof: Default::default(),
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomX,
            pow_data: serialized,
        };
        block_header.pow = pow;
        let err = verify_header(&block_header, &block_header.hash(), &rules).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("Expected merge mining tag was not found in Monero coinbase transaction"));
    }

    #[test]
    fn test_verify_invalid_root() {
        let rules = ConsensusManager::builder(Network::LocalNet).build().unwrap();
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
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 0,
        };
        let hash = block_header.merge_mining_hash();
        insert_aux_chain_mr_and_info_into_block(&mut block, hash, 1, 0).unwrap();
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
        let aux_hashes = vec![monero::Hash::from_slice(hash.as_ref())];
        let aux_chain_merkle_proof = create_merkle_proof(&aux_hashes, &aux_hashes[0]).unwrap();

        let coinbase = block.miner_tx.clone();
        let extra = coinbase.prefix.extra.clone();
        let mut keccak = Keccak::v256();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
        keccak.update(&encoder_prefix);

        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_canonical_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: count,
            merkle_root: Hash::null(),
            coinbase_merkle_proof,
            coinbase_tx_hasher: keccak,
            coinbase_tx_extra: extra,
            aux_chain_merkle_proof,
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomX,
            pow_data: serialized,
        };
        block_header.pow = pow;
        let err = verify_header(&block_header, &block_header.hash(), &rules).unwrap_err();
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

    #[test]
    fn test_extra_field_deserialize() {
        let bytes = vec![
            3, 33, 0, 149, 5, 198, 66, 174, 39, 113, 243, 68, 202, 221, 222, 116, 10, 209, 194, 56, 247, 252, 23, 248,
            28, 44, 81, 91, 44, 214, 211, 242, 3, 12, 70, 0, 0, 0, 1, 251, 88, 0, 0, 96, 49, 163, 82, 175, 205, 74,
            138, 126, 250, 226, 106, 10, 255, 139, 49, 41, 168, 110, 203, 150, 252, 208, 234, 140, 2, 17, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let raw_extra_field = RawExtraField(bytes);
        let res = ExtraField::try_parse(&raw_extra_field);
        assert!(res.is_err());
        let field = res.unwrap_err();
        let mm_tag = SubField::MergeMining(
            Some(VarInt(0)),
            Hash::from_slice(
                hex::decode("9505c642ae2771f344caddde740ad1c238f7fc17f81c2c515b2cd6d3f2030c46")
                    .unwrap()
                    .as_slice(),
            ),
        );
        assert_eq!(field.0[0], mm_tag);
    }
}

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

use std::{
    fmt,
    fmt::{Display, Formatter},
    io,
    io::Write,
};

use borsh::{BorshDeserialize, BorshSerialize};
use monero::{
    blockdata::transaction::RawExtraField,
    consensus::{Decodable, Encodable},
    cryptonote::hash::Hashable,
    util::ringct::{RctSigBase, RctType},
};
use tari_utilities::{
    hex::{to_hex, Hex},
    ByteArray,
};
use tiny_keccak::{Hasher, Keccak};

use super::{error::MergeMineError, fixed_array::FixedByteArray, merkle_tree::MerkleProof};
use crate::{
    blocks::BlockHeader,
    consensus::ConsensusManager,
    proof_of_work::monero_rx::helpers::create_block_hashing_blob,
};

/// This is a struct to deserialize the data from he pow field into data required for the randomX Monero merged mine
/// pow.
#[derive(Clone, Debug)]
pub struct MoneroPowData {
    /// Monero header fields
    pub header: monero::BlockHeader,
    /// RandomX vm key - the key length varies to a maximum length of 60. We'll allow a up to 63 bytes represented in
    /// fixed 64-byte struct (63 bytes + 1-byte length).
    pub randomx_key: FixedByteArray,
    /// The number of transactions included in this Monero block. This is used to produce the blockhashing_blob
    pub transaction_count: u16,
    /// Transaction root
    pub merkle_root: monero::Hash,
    /// Coinbase merkle proof hashes
    pub coinbase_merkle_proof: MerkleProof,
    /// incomplete hashed state of the coinbase transaction
    pub coinbase_tx_hasher: Keccak,
    /// extra field of the coinbase
    pub coinbase_tx_extra: RawExtraField,
    /// aux chain merkle proof hashes
    pub aux_chain_merkle_proof: MerkleProof,
}

impl BorshSerialize for MoneroPowData {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        self.header.consensus_encode(writer)?;
        BorshSerialize::serialize(&self.randomx_key, writer)?;
        BorshSerialize::serialize(&self.transaction_count, writer)?;
        self.merkle_root.consensus_encode(writer)?;
        BorshSerialize::serialize(&self.coinbase_merkle_proof, writer)?;
        BorshSerialize::serialize(&self.coinbase_tx_hasher, writer)?;
        BorshSerialize::serialize(&self.coinbase_tx_extra.0, writer)?;
        BorshSerialize::serialize(&self.aux_chain_merkle_proof, writer)?;
        Ok(())
    }
}

impl BorshDeserialize for MoneroPowData {
    fn deserialize_reader<R>(reader: &mut R) -> Result<Self, io::Error>
    where R: io::Read {
        let header = monero::BlockHeader::consensus_decode(reader)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let randomx_key = BorshDeserialize::deserialize_reader(reader)?;
        let transaction_count = BorshDeserialize::deserialize_reader(reader)?;
        let merkle_root = monero::Hash::consensus_decode(reader)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let coinbase_merkle_proof = BorshDeserialize::deserialize_reader(reader)?;
        let coinbase_tx_hasher = BorshDeserialize::deserialize_reader(reader)?;
        let coinbase_tx_extra = RawExtraField(BorshDeserialize::deserialize_reader(reader)?);
        let aux_chain_merkle_proof = BorshDeserialize::deserialize_reader(reader)?;
        Ok(Self {
            header,
            randomx_key,
            transaction_count,
            merkle_root,
            coinbase_merkle_proof,
            coinbase_tx_hasher,
            coinbase_tx_extra,
            aux_chain_merkle_proof,
        })
    }
}

impl MoneroPowData {
    /// Create a new MoneroPowData struct from the given header
    pub fn from_header(
        tari_header: &BlockHeader,
        consensus: &ConsensusManager,
    ) -> Result<MoneroPowData, MergeMineError> {
        let mut v = tari_header.pow.pow_data.as_bytes();
        let pow_data: MoneroPowData =
            BorshDeserialize::deserialize(&mut v).map_err(|e| MergeMineError::DeserializeError(format!("{:?}", e)))?;
        if pow_data.coinbase_tx_extra.0.len() > consensus.consensus_constants(tari_header.height).max_extra_field_size()
        {
            return Err(MergeMineError::DeserializeError(format!(
                "Extra size({}) is larger than allowed {} bytes",
                pow_data.coinbase_tx_extra.0.len(),
                consensus.consensus_constants(tari_header.height).max_extra_field_size()
            )));
        }
        if !v.is_empty() {
            return Err(MergeMineError::DeserializeError(format!(
                "{} bytes leftover after deserialize",
                v.len()
            )));
        }
        let mut test_serialized_data = vec![];

        // This is an inefficient test, so maybe it can be removed in future, but because we rely
        // on third party parsing libraries, there could be a case where the data we deserialized
        // can be generated from multiple input data. This way we test that there is only one of those
        // inputs that is allowed. Remember that the data in powdata is used for the hash, so having
        // multiple pow_data that generate the same randomx difficulty could be a problem.
        BorshSerialize::serialize(&pow_data, &mut test_serialized_data)
            .map_err(|e| MergeMineError::SerializeError(format!("{:?}", e)))?;
        if test_serialized_data != tari_header.pow.pow_data.to_vec() {
            return Err(MergeMineError::SerializedPowDataDoesNotMatch(
                "Serialized pow data does not match original pow data".to_string(),
            ));
        }

        Ok(pow_data)
    }

    /// Returns true if the coinbase merkle proof produces the `merkle_root` hash, otherwise false
    pub fn is_coinbase_valid_merkle_root(&self) -> bool {
        let mut finalised_prefix_keccak = self.coinbase_tx_hasher.clone();
        let mut encoder_extra_field = Vec::new();
        self.coinbase_tx_extra
            .consensus_encode(&mut encoder_extra_field)
            .unwrap();
        finalised_prefix_keccak.update(&encoder_extra_field);
        let mut prefix_hash: [u8; 32] = [0; 32];
        finalised_prefix_keccak.finalize(&mut prefix_hash);

        let final_prefix_hash = monero::Hash::from_slice(&prefix_hash);

        // let mut finalised_keccak = Keccak::v256();
        let rct_sig_base = RctSigBase {
            rct_type: RctType::Null,
            txn_fee: Default::default(),
            pseudo_outs: vec![],
            ecdh_info: vec![],
            out_pk: vec![],
        };
        let hashes = vec![final_prefix_hash, rct_sig_base.hash(), monero::Hash::null()];
        let encoder_final: Vec<u8> = hashes.into_iter().flat_map(|h| Vec::from(&h.to_bytes()[..])).collect();
        let coinbase_hash = monero::Hash::new(encoder_final);

        let merkle_root = self.coinbase_merkle_proof.calculate_root(&coinbase_hash);
        (self.merkle_root == merkle_root) && self.coinbase_merkle_proof.check_coinbase_path()
    }

    /// Returns the blockhashing_blob for the Monero block
    pub fn to_blockhashing_blob(&self) -> Vec<u8> {
        create_block_hashing_blob(&self.header, &self.merkle_root, u64::from(self.transaction_count))
    }

    /// Returns the RandomX vm key
    pub fn randomx_key(&self) -> &[u8] {
        self.randomx_key.as_slice()
    }
}

impl Display for MoneroPowData {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        writeln!(fmt, "MoneroBlockHeader: {} ", self.header)?;
        writeln!(fmt, "RandomX vm key: {}", self.randomx_key.to_hex())?;
        writeln!(fmt, "Monero tx count: {}", self.transaction_count)?;
        writeln!(fmt, "Monero tx root: {}", to_hex(self.merkle_root.as_bytes()))
    }
}

#[cfg(test)]
mod test {
    use borsh::{BorshDeserialize, BorshSerialize};
    use chacha20poly1305::aead::OsRng;
    use monero::{blockdata::transaction::RawExtraField, consensus::Encodable, BlockHeader, Hash, VarInt};
    use tari_common::configuration::Network;
    use tari_common_types::types::PrivateKey;
    use tari_crypto::keys::SecretKey;
    use tari_utilities::ByteArray;
    use tiny_keccak::{Hasher, Keccak};

    use super::MoneroPowData;
    use crate::{
        consensus::NetworkConsensus,
        proof_of_work::{
            monero_rx::{merkle_tree::MerkleProof, FixedByteArray},
            PowData,
        },
    };

    #[test]
    fn test_borsh_de_serialization() {
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

        let monero_pow_data = MoneroPowData {
            header: BlockHeader {
                major_version: VarInt(1),
                minor_version: VarInt(2),
                timestamp: VarInt(3),
                prev_id: Hash::new([4; 32]),
                nonce: 5,
            },
            randomx_key: FixedByteArray::from_canonical_bytes(&[6, 7, 8]).unwrap(),
            transaction_count: 9,
            merkle_root: Hash::new([10; 32]),
            coinbase_merkle_proof: MerkleProof::default(),
            coinbase_tx_extra: extra,
            coinbase_tx_hasher: keccak,
            aux_chain_merkle_proof: MerkleProof::default(),
        };
        let mut buf = Vec::new();
        monero_pow_data.serialize(&mut buf).unwrap();
        buf.extend_from_slice(&[1, 2, 3]);
        let buf = &mut buf.as_slice();
        MoneroPowData::deserialize(buf).unwrap();
        assert_eq!(buf, &[1, 2, 3]);
    }

    #[test]
    fn max_monero_pow_data_bytes_fits_inside_proof_of_work_pow_data() {
        let coinbase: monero::Transaction = Default::default();
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

        for network in [
            Network::MainNet,
            Network::StageNet,
            Network::LocalNet,
            Network::NextNet,
            Network::Igor,
            Network::Esmeralda,
        ] {
            for consensus_constants in NetworkConsensus::from(network).create_consensus_constants() {
                let monero_pow_data = MoneroPowData {
                    header: BlockHeader {
                        major_version: VarInt(u64::MAX),
                        minor_version: VarInt(u64::MAX),
                        timestamp: VarInt(u64::MAX),
                        prev_id: Hash::new(PrivateKey::random(&mut OsRng).to_vec()),
                        nonce: u32::MAX,
                    },
                    randomx_key: FixedByteArray::default(),
                    transaction_count: u16::MAX,
                    merkle_root: Hash::new(PrivateKey::random(&mut OsRng).to_vec()),
                    coinbase_merkle_proof: MerkleProof::default(),
                    coinbase_tx_extra: RawExtraField(vec![1u8; consensus_constants.max_extra_field_size()]),
                    coinbase_tx_hasher: keccak.clone(),
                    aux_chain_merkle_proof: MerkleProof::default(),
                };
                let mut buf = Vec::new();
                monero_pow_data.serialize(&mut buf).unwrap();
                assert!(buf.len() <= PowData::default().max_size());
            }
        }
    }
}
